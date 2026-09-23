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

**token 默认必需。** 首次启动时服务端把 32 字节随机值写入 `<data-dir>/token`（仅属主可读），此后不带它的请求一律拒绝，所以请读出该文件并原样发出：

```bash
export RISCDOM_TOKEN="$(cat /path/to/data-dir/token)"
curl -sS http://127.0.0.1:7821/v0/health -H "Authorization: Bearer $RISCDOM_TOKEN"
```

token **从不被打印或记入日志**——启动行只报文件路径，不报值——所以文件是拿到它的唯一地方。部署方也可以自行放置该文件。`--no-auth` 取消该要求（会打印警告）：仅用于本地调试，因为控制端点里含破坏性操作。`401` 表示凭证缺失或错误；`403` 表示凭证已认，但它的 actor 不持有该端点的 capability（见下）。

两个无需参数的调用是存活检查与概要：

```bash
curl -sS http://127.0.0.1:7821/v0/health
# {"status":"ok","version":"0.8.0","uptime_ms":1971}

curl -sS http://127.0.0.1:7821/v0/status
# {"agents":1,"agent_id":"local-17480-1","connections":1,"sse_subscribers":0,
#  "status":"ok","uptime_ms":1997,"version":"0.8.0"}
```

`connections` 是打开的 TCP 连接数；`sse_subscribers` 是存活的事件流数；`agents` 是本宿主知道的 agent 数。

### capability：一份凭证能做什么

认证与授权是两个决定。`401` 表示服务端不接受该凭证；`403` 表示它接受了，但它解析出的 actor 不被允许做**这件事**。

每个端点要求一个 capability，名字见 API 文档 §5 表格——共 28 个，例如 `agent.run`、`audit.read`、`runs.control`、`settings.write`、`vm.control`。服务端在运行处理器前检查，客户端因此可以事先规划，而不是撞上才知道：

```bash
# token 持有者持有全部 28 项，此调用成功。
curl -sS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:7821/v0/status \
  -H "Authorization: Bearer $RISCDOM_TOKEN"
# 200

# 完全没凭证：在考虑 capability 之前就被拒。
curl -sS http://127.0.0.1:7821/v0/status
# {"code":"unauthorized","message":"missing or invalid bearer token","retryable":false,"cause":null}
# 401

# 通过了认证，但 actor 不持有该 capability。
curl -sS -X POST http://127.0.0.1:7821/v0/sessions/clear \
  -H "Authorization: Bearer $RISCDOM_TOKEN"
# {"code":"forbidden","message":"the actor may not session.write","retryable":false,"cause":"capability"}
# 403
```

第三种在 v0.9 默认下不会出现：token 持有者持有全部，`NoAuth`（`--no-auth`）发放同一集合。它出现在分发方自己的 `Authn` 钩子返回更窄 actor 时——也正是「认证通过不等于获准」、客户端要读 `cause` 而不是凭假设的原因。规则是默认拒绝：除非凭证的 actor 恰好持有该路由声明的 capability，该端点一律拒绝。

### 一个安全的部署

回环默认不是走过场：这个构建说的是明文 HTTP。再往外走一步的部署里，有三件事属于它。

1. **token 文件只给属主。** `<data-dir>/token` 在 Unix 上以 `600` 写入，在 Windows 上带仅属主 ACL；收紧不了时服务端拒绝启动。备份、拷贝、挂载时保持这些权限，也让该值远离共享的 shell 历史（`$(cat …)` 同时让它不进 `ps`）。
2. **绑得窄，再在前面加 TLS。** 保持 `--bind 127.0.0.1:7821`，让反向代理负责对外。终止 TLS 是代理的事；服务端没有 TLS。
3. **不要把 `--no-auth` 与非回环绑定放在一起。** 那等于「谁能连上这个端口，谁就能删会话、停 VM」。

一份最小 nginx 前置示例（示意——本构建不带代理）：

```nginx
server {
    listen 443 ssl;
    server_name riscdom.example.internal;
    ssl_certificate     /etc/ssl/riscdom/fullchain.pem;
    ssl_certificate_key /etc/ssl/riscdom/privkey.pem;

    location /v0/ {
        proxy_pass http://127.0.0.1:7821;
        # bearer token 就是凭证；让它留在这一跳。
        proxy_set_header Authorization $http_authorization;
        proxy_set_header Host $host;
        # SSE：不缓冲、不设空闲超时、用 HTTP/1.1。
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_buffering off;
        proxy_read_timeout 1h;
    }
}
```

`proxy_buffering off` 与较长的 `proxy_read_timeout`，是让 `/v0/events` 保持一条活的流、而不是在第一个心跳迟到时就结束的请求的关键。

## 2. 查询端点

共 26 个，全部 `GET`。应答是宿主的视图类型，字段见 `host-core/src/state.rs`。一律 JSON。

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
| 403 | `forbidden` | 不被允许：`cause` 为 `"capability"` 表示 actor 缺少该端点的 capability，否则是路径越出 workspace。不要重试。 |
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

**重连。** 每帧都带 `id: <ts>-<seq>`。记住你见过的最后一个，并把它作为 `Last-Event-ID` 回传；服务端会从内存缓冲（最近 1024 帧）补发其后的帧：

```bash
curl -sS -N http://127.0.0.1:7821/v0/events \
  -H "Authorization: Bearer $RISCDOM_TOKEN" \
  -H 'Last-Event-ID: 1790074877033-7'
```

若游标已掉出缓冲，事件流会先发一个 `gap` 帧——`payload.lost_after` 给出仍持有的最旧 id——随后是从那里起的帧。无论哪种情况，都请用上面的查询重新读取状态，而不要假定事件流是完整的。

## 6. 控制端点

27 个 `POST` 端点，即 API 表的 §5.2。它们全部需要 token（这正是引入它们的那一批的要点：其中包含破坏性操作）。

```bash
# 跑一轮 agent。进度以 `agent:*` 事件抵达事件流。
curl -sS -X POST http://127.0.0.1:7821/v0/agent/run \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"user_input":"compile the blink example"}'

# 会话。
curl -sS -X POST http://127.0.0.1:7821/v0/sessions/create \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"title":"blink"}'
# {"session_id":"sess-1a2b3c4d-0"}

curl -sS -X POST http://127.0.0.1:7821/v0/sessions/clear \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' -d '{}'

# 快照。
curl -sS -X POST http://127.0.0.1:7821/v0/snapshots/save \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"name":"after-blink"}'
# {"bytes_written":1048576}

curl -sS -X POST http://127.0.0.1:7821/v0/snapshots/resume \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"name":"after-blink"}'

# VM、设置、导出。
curl -sS -X POST http://127.0.0.1:7821/v0/vm/stop \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' -d '{}'

curl -sS -X POST http://127.0.0.1:7821/v0/settings/theme \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"theme":"dark"}'

curl -sS -X POST http://127.0.0.1:7821/v0/audit/export \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"path":"/abs/path/inside/the/workspace/audit.jsonl"}'
```

客户端应当知道的几点：

- **多数控制端点回 `204`**（无内容），返回值的回 `200`，而后台开工的（`agent/run`、`preflight/run`、`toolchain/download`）回 `202`——这些请盯事件流。
- **参数会被校验**：缺失或不可用即 `400`，`cause` 指出参数名。
- **状态冲突回 `409`**（没跑 VM 时 `save`、`resume` 未知快照回 `404`、没有下载在跑时 `toolchain/download/cancel`）。
- **`POST /v0/toolchain/download` 会真的下载**固定的 RISC-V GCC 归档。
- **`POST /v0/vm/start` 回 `501`**（预留：今天 VM 在运行内部启动），`GET /v0/resources` 同样。

## 7. 用 CLI 驱动控制平面

`riscdom` 是参考客户端，也是「这个控制平面到底答不答本文档所说的东西」最快的核对方式。它是严格意义上的客户端：每条命令都是一次 HTTP 请求，而本地模式只不过是在自己进程内、由内核挑一个回环端口把控制平面起起来。

```bash
riscdom health --json                 # 对着它自己起的控制平面
riscdom --json --remote 127.0.0.1:7821 runs list --limit 5   # 对着已经跑着的那个
```

| 子命令 | 端点 |
|---|---|
| `riscdom health` | `GET /v0/health` |
| `riscdom status` / `riscdom agents` | `GET /v0/status` |
| `riscdom runs list [--limit <n>]` | `GET /v0/runs` |
| `riscdom runs get <run_id>` | `GET /v0/runs/<run_id>` |
| `riscdom audit status` | `GET /v0/audit/status` |
| `riscdom audit events [--limit <n>]` | `GET /v0/audit/events` |
| `riscdom snapshots list` | `GET /v0/snapshots` |

- **`--json`** 打印的就是控制平面发来的原文——§2、§5 记录的那些字段——因此照着本文档写出的客户端可以用它来调试。失败时把 §4 的错误体打到 **stderr**。
- **退出码**把 §4 的状态码变成脚本可分叉的东西：`0` 成功、`1` 本地失败（连不上、没有 token）、`2` 用法或 `400`、`3` 被拒或 `5xx`、`4` `401`/`403`。
- **token**：本地模式取自 `<data-dir>/token`；远程模式按 `--token-file`、`RISCDOM_TOKEN`、`--token` 的顺序取。从不被打印。

完整表格（含人类模式形状）见 [../cli/README.zh-CN.md](../cli/README.zh-CN.md)。

## 8. 还没有的东西

- **细粒度凭证。** 每条路由的 capability 都已强制（见 §1）；v0.9 缺的只是不止一种凭证。单个 token 持有一切，因此没法只授予「只读审计链」的客户端——按能力细分的 token 属 v1.0。
- **`POST /v0/vm/start`** 与 **`GET /v0/resources`** 回 `501`。
