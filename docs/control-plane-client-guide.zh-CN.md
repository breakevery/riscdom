[English](control-plane-client-guide.md) | 中文

# 编写控制平面客户端

> **适用 v0.9（v1.0 前不稳定）。** 本指南写给**写客户端的人**：如何调用查询端点、如何读应答、如何处理错误、以及如何订阅事件流。规范表格在 [control-plane-api.zh-CN.md](control-plane-api.zh-CN.md) 与 [control-plane-events.zh-CN.md](control-plane-events.zh-CN.md)；本文是可照着做的走查。

这里全部是普通 HTTP。没有 SDK、没有生成式客户端：接口面就是 HTTP 接口，所以 `curl`、`fetch`、或任何 HTTP 库都够用。

## 1. 第一次调用之前

把 server 指向你要查看的 workspace 启动（参数见 [../server/README.zh-CN.md](../server/README.zh-CN.md)）：

```bash
riscdom-server --bind 127.0.0.1:7821 --workspace ./my-workspace
```

每个请求都带凭证：

```bash
curl -sS http://127.0.0.1:7821/v0/health \
  -H 'Authorization: Bearer <token>'
```

v0.9 默认钩子（`NoAuth`）放行所有请求并忽略该头，所以不带也能成功——但从一开始就带上，部署了真钩子时才不会让客户端措手不及。`401` 表示钩子拒绝了凭证；`403` 表示它认了调用者但拒绝了动作。

两个无需参数的调用是存活检查与概要：

```bash
curl -sS http://127.0.0.1:7821/v0/health
# {"status":"ok","version":"0.8.0","uptime_ms":1971}

curl -sS http://127.0.0.1:7821/v0/status
# {"agents":1,"agent_id":"local-17480-1","connections":1,"sse_subscribers":0,
#  "status":"ok","uptime_ms":1997,"version":"0.8.0"}
```

`connections` 是打开的 TCP 连接数；`sse_subscribers` 是存活的事件流数；`agents` 是本宿主知道的 agent 数。

## 2. 查询端点

共 26 个，全部 `GET`。应答是宿主的视图类型，字段见 `host/src/state.rs`。一律 JSON。

### 审计与运行

```bash
# 事件总数，以及哈希链是否完整。
curl -sS 'http://127.0.0.1:7821/v0/audit/status'
# {"alert_on_failure":true,"chain":{"status":"Intact","length":42},"count":42,"failures":[]}

# 最近的事件，新的在前。这里 limit 是必填。
curl -sS 'http://127.0.0.1:7821/v0/audit/events?limit=10'
curl -sS 'http://127.0.0.1:7821/v0/audit/events?limit=10&actor=local-17480-1'
curl -sS 'http://127.0.0.1:7821/v0/audit/events?limit=10&action_prefix=agent.tool'

# 派生索引里的运行（limit 默认 20）。
curl -sS 'http://127.0.0.1:7821/v0/runs?limit=20'

# 单个运行；本日志从未见过它时回 null。
curl -sS 'http://127.0.0.1:7821/v0/runs/run-17480-3'

# 两次运行的配置指纹，逐字段对比。
curl -sS 'http://127.0.0.1:7821/v0/runs/diff?run_a=run-17480-3&run_b=run-17480-4'
```

`RunView` 携带 `run_id`、`status`（`open` / `ok` / `failed` / `interrupted` / `abandoned`）、`fingerprint`（64 位十六进制）与 `fingerprint_short`（前 16 位）、`parent_run_id`、`session_id`、`resumed_from_snapshot`、`started_at_ms` 与 `ended_at_ms`。

> **`/v0/audit/status` 不消费失败队列。** 桌面端命令会取走待报的审计失败；这个 `GET` 只报告、不取走，因此在这里轮询不会偷走别的客户端的告警。

> **`/v0/runs/diff` 遇到不存在的 run 回 `500 internal`。** 宿主把「找不到 run」报成消息而不是有类型的 not-found。需要区分「没有这个 run」与「diff 失败」时，用 `/v0/runs/{id}` 先确认。

### LLM 配置

这里任何响应都不会返回 key。

```bash
curl -sS 'http://127.0.0.1:7821/v0/llm/provider-presets'   # 下拉框的数据
curl -sS 'http://127.0.0.1:7821/v0/llm/config'             # {"configured":false,...}
curl -sS 'http://127.0.0.1:7821/v0/llm/readiness'          # {"ready":false,"reason":"no_config",...}
curl -sS 'http://127.0.0.1:7821/v0/llm/local-probe'        # 回环上的 OpenAI 兼容服务
curl -sS 'http://127.0.0.1:7821/v0/llm/stored-key?provider_id=deepseek'
# {"present":false}
```

### 会话、快照、VM

```bash
curl -sS 'http://127.0.0.1:7821/v0/sessions?limit=20'   # limit 必填
curl -sS 'http://127.0.0.1:7821/v0/sessions/current'
# {"session_id":"s-17480-1"}   （或 null）

curl -sS 'http://127.0.0.1:7821/v0/snapshots'
# [{"name":"after-blink","size_bytes":1048576,"created_at_ms":1758533002099,"mode":"tcp-relay"}]

curl -sS 'http://127.0.0.1:7821/v0/vm/running'   # {"running":true}
curl -sS 'http://127.0.0.1:7821/v0/vm/status'    # {"running":true,"since_ms":1758533002050}
```

快照的 `mode` 为 `tcp-relay`（真实迁移流）或 `reboot-fallback`（JSON 兜底）。

### 工具链、QEMU、预检

```bash
curl -sS 'http://127.0.0.1:7821/v0/toolchain'
# {"found":true,"path":"...","source":"Path","diagnostics":"..."}
curl -sS 'http://127.0.0.1:7821/v0/toolchain/download'
# {"in_progress":false,"last_event":null}
curl -sS 'http://127.0.0.1:7821/v0/qemu'
curl -sS 'http://127.0.0.1:7821/v0/qemu/status'   # 同一个视图
curl -sS 'http://127.0.0.1:7821/v0/preflight'
# {"ran":true,"checked":true,"ok":true,"rows":[...],"failed_step":null,...}
```

`source` 取 `EnvVar` / `KnownPath` / `Path` / `Manual` 之一。预检应答是当前配置的缓存结果：端点只报告，不跑检查。

### 设置、workspace、串口

```bash
curl -sS 'http://127.0.0.1:7821/v0/settings/theme'      # {"theme":"system"}
curl -sS 'http://127.0.0.1:7821/v0/settings/language'   # {"language":"system"}
curl -sS 'http://127.0.0.1:7821/v0/workspace/root'      # {"root":"/abs/path/to/workspace"}
curl -sS 'http://127.0.0.1:7821/v0/workspace/files'     # ["src/main.c", ...]
curl -sS 'http://127.0.0.1:7821/v0/workspace/file?path=src%2Fmain.c'
# {"content":"int main(void) { return 0; }\n"}
curl -sS 'http://127.0.0.1:7821/v0/serial'              # {"buffer":"hello from riscv\n"}
```

`?path=` 会做百分号解码，所以 `src%2Fmain.c` 与 `src/main.c` 是同一个请求。读取会过 workspace 策略检查：越出根目录即 `403`。

### 预留的聚合端点

```bash
curl -sS 'http://127.0.0.1:7821/v0/resources'
# {"code":"not_implemented","message":"resource accounting is reserved ...","retryable":false,"cause":"resources"}
```

## 3. 参数

- 必填参数缺失或无法解析即 `400`，`cause` 为该参数名：

```json
{ "code": "bad_request", "message": "missing required parameter \"limit\"", "retryable": false, "cause": "limit" }
```

- `limit` 在宿主命令要求处为必填：`/v0/audit/events` 与 `/v0/sessions`。`/v0/runs` 可选，默认 20。
- v0.9 没有任何 offset 或游标：调大 `limit`，在客户端侧过滤。

## 4. 处理错误

每个非 2xx 应答都是一个对象：

```json
{ "code": "not_found", "message": "no endpoint GET /v0/nope", "retryable": false, "cause": null }
```

把 `code` 当契约、`message` 当给人看的文本。客户端应对 `code` 分支，最多再读 `cause` 定位出错的字段。

| 状态 | `code` | 该怎么做 |
|---|---|---|
| 400 | `bad_request` | 改请求；`cause` 指出参数名。 |
| 401 | `unauthorized` | 带有效凭证。 |
| 403 | `forbidden` | 调用者或路径不被允许；不要重试。 |
| 404 | `not_found` | 端点或资源不存在。 |
| 405 | `method_not_allowed` | 用 `message` 里指出的方法。 |
| 409 | `conflict` | 状态冲突；重读状态再决定。 |
| 500 | `internal` | 宿主失败；`message` 是宿主自己的文本。 |
| 501 | `not_implemented` | 预留；后续批次补齐。 |
| 503 | `unavailable` | 依赖未就绪（无 LLM、无 QEMU、无工具链）。 |

仅在 `retryable` 为 `true` 时重试——异步工作由控制类（`202` + 事件流）自行报告，查询面里没有这种情况。

```bash
# 缺少必填参数。
curl -sS -o - -w '\n%{http_code}\n' 'http://127.0.0.1:7821/v0/audit/events'
# {"code":"bad_request","message":"missing required parameter \"limit\"","retryable":false,"cause":"limit"}
# 400

# 已服务的路径上方法不对。
curl -sS -o - -w '\n%{http_code}\n' -X POST 'http://127.0.0.1:7821/v0/snapshots'
# {"code":"method_not_allowed","message":"POST is not allowed on /v0/snapshots; use GET","retryable":false,"cause":"method"}
# 405
```

## 5. 订阅事件流

`GET /v0/events` 是 `text/event-stream`。它会一直开着，所以用会流式读的客户端：

```bash
curl -sS -N http://127.0.0.1:7821/v0/events \
  -H 'Authorization: Bearer <token>' \
  -H 'Accept: text/event-stream'
```

第一帧恒为 `hello`；其后是宿主的事件，一事件一帧：

```text
id: 1790074876659-0
data: {"version":1,"kind":"hello","event":null,"agent_id":"local-17480-1","task_id":null,"ts":1790074876659,"payload":{"buffer":{"from":0,"to":0},"filters":{"event":[],"agent_id":null,"task_id":null}}}

id: 1790074877033-1
data: {"version":1,"kind":"event","event":"agent:tool_call","agent_id":"local-17480-1","task_id":null,"ts":1790074877033,"payload":{"name":"write_source","arguments":{"path":"src/main.c"}}}

id: 1790074877100-2
data: {"version":1,"kind":"event","event":"serial:chunk","agent_id":"local-17480-1","task_id":null,"ts":1790074877100,"payload":{"chunk":"hello from riscv\n"}}

```

客户端可以依赖的规则：

- **帧只用 `id:` 与 `data:`。** 没有 SSE 的 `event:` 字段，所以每帧都进同一个 `onmessage`；按 envelope 的 `event` 路由。
- **每帧以空行结束。**
- **心跳是注释行**，每 15 秒一次：`: keep-alive`。以 `:` 开头的行忽略。
- **`id:` 不透明。** 打算重连就存下它；不要解析。
- **envelope 的键在 v0.9 内稳定**：`version`、`kind`、`event`、`agent_id`、`task_id`、`ts`、`payload`。忽略不认识的 payload 键。

```js
const source = new EventSource("/v0/events"); // 已建立会话
source.onmessage = (e) => {
  const frame = JSON.parse(e.data);
  if (frame.kind !== "event") return;          // hello（以及日后的 gap）
  switch (frame.event) {
    case "vm:state":
      // `name` 现在恒存在：非快照时为 null。
      badge(frame.payload.running, frame.payload.since_ms, frame.payload.name);
      break;
    case "serial:chunk":
      terminal.write(frame.payload.chunk);
      break;
    case "audit:failed":
      warn(frame.payload.message);              // v0.9 之前叫 `error`
      break;
  }
};
```

**尚未实现：**`gap` 帧与 `Last-Event-ID` 补放。落后的订阅者会丢掉错过的帧且不会被通知；请重连并用上面的查询重新读取状态，而不要假定事件流是完整的。

## 6. 还没有的东西

- **控制类**（`POST`，API 表的 §5.2）：HTTP 上还没有 `agent:run`、没有保存快照、没有会话写入。
- **`gap` 与 `Last-Event-ID`**，如上。
- **权限强制。** 服务端标注每个端点的权限并交给认证钩子；判断调用者**是否持有**该权限属后续批次，因此 v0.9 只在钩子直接拒绝时回 `403`。
- **`/v0/resources`** 回 `501`。
